---
id: F10
title: Vue « Fichiers les plus anciens » / répartition par âge (F10)
model: sonnet        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Une vue de plus dans le cadre F07/F09 (variante de `View`, réutilisation de `TopView` par un paramètre) et des fonctions pures courtes dont les bornes, la conversion minuit local vers UTC et les valeurs de test sont données ici ; rien ne touche au scan ni au stockage.
depends_on: [F02]
---

## Contexte

Après les gros fichiers (F09), l'autre question classique d'un analyseur d'espace est « qu'est-ce qui dort là depuis des années ? ». TreeSize a un onglet « Âge des fichiers » (répartition de l'espace par tranche d'âge) ; WinDirStat / WizTree permettent de trier par date. Le menu **Affichage** contient aujourd'hui une entrée grisée « Fichiers les plus anciens ».

État actuel utile (vérifié dans le code) :

- `enum View { Details, Map, Types, Top }` (`src/ui/mod.rs`), `impl View` dans `src/ui/views.rs` (`ALL` à 4 éléments, `label`, `tab`, `key`, `key_text`, `for_key`, `side_treemap`, `side_types`). Le commentaire de `ALL` dit « F10 ajoutera sa variante ici ». F6–F9 passent par `View::ALL` dans `App::shortcuts` (sans modificateur, même si un champ texte a le focus).
- **Vue Top (F09)**, `src/ui/top.rs` : `top_files` (tas borné), `rel_dir`, `needs_rebuild` (500 ms pendant le scan), `TopView { rows, built_version, built, sort, desc, scroll_to, scroll_y, view_h }` avec `new`, `reset`, `rebuild`, `apply_sort`, `click_col`, `show` (titre, en-tête triable, lignes virtualisées, flèches, double-clic = révéler, pied) et `Layout`. Colonnes : #, Nom, Dossier, Taille, %, Modifié, Type. **Cette carte réutilise `TopView` tel quel en lui ajoutant un paramètre « quelle liste », pas une copie.**
- `App` (`src/app.rs`) : champ `top: TopView`, `start_scan` appelle `self.top.reset()` ; branche `View::Top` du panneau central dans `Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 12 })` ; `revealed` (double-clic) → `selected`, `tv.reveal`, `Action::SetView(View::Details)`. `now` = secondes Unix UTC, calculé à chaque frame.
- **Dates** : le scan ne lit que la date de **modification** (`FILE_ID_BOTH_DIR_INFO.write` dans `src/scan/win.rs`, `metadata().modified()` dans le repli `scan_dir_std`) → `Entry::mtime` / `Node::mtime`, secondes Unix UTC en `u32`. `filetime_to_unix` borne à `[0, u32::MAX]` : **`0` = inconnue, illisible ou antérieure à 1970**. Les dossiers ont aussi un `mtime`.
- `src/fmt.rs` (F02) : `pub fn date(secs: u32, now: u64)` ; `fn local(utc: i64) -> i64` (UTC → secondes « Unix locales », fuseau actif avec l'heure d'été **de la date convertie**) est **privée** ; `local_with(tz, utc)` sert aux tests Windows. `local` appelle trois fonctions kernel32 : **trop cher pour 1,3 M d'appels par recalcul** (voir Risques), d'où les bornes précalculées ci-dessous.
- Le treemap de la colonne droite et la carte Types restent affichés pour toute vue autre que Carte / Types.
- Fichiers de l'utilisateur observés dans la vue F09 : des dates affichées « 1 janv. 1970 » (horodatage de quelques secondes après l'époque Unix, typique des archives et constructions « reproductibles »). Elles ne doivent ni encombrer la liste des plus anciens, ni être comptées comme « plus de 10 ans ».

### Touche F10 sous Windows (vérifié dans les sources)
Dans une application Win32 classique, F10 (comme Alt) arrive en `WM_SYSKEYDOWN` et `DefWindowProc` active la barre de menus (`SC_KEYMENU`), ce qui avale la touche suivante. **winit 0.30.13** (version de `Cargo.lock`) ne transmet pas `WM_SYSKEYDOWN` / `WM_SYSKEYUP` à `DefWindowProc` : `public_window_callback_inner` (`event_loop.rs`) et `KeyEventBuilder::process_message` (`keyboard.rs`) fixent `ProcResult::Value(0)` pour ces messages (seul Alt+F4 est traité à part). F10 arrive donc à egui comme `Key::F10`, sans mode menu. **Choix : F10**, dans la suite F6–F9. À confirmer à l'écran (Vérification manuelle, point 2) ; si F10 était malgré tout intercepté, ne pas changer de touche dans cette tâche : le noter dans « Limites » du README (l'onglet et le menu restent utilisables).

## Comportement attendu

### Décisions
- **Une seule vue** `View::Old`, deux parties empilées : en haut la **répartition par âge** (histogramme horizontal par tranche), en dessous la **liste des 100 fichiers les plus anciens** (même tableau que la vue Top). Pas de deuxième vue ni de deuxième touche (la touche F11 est réservée par convention au plein écran ; la recherche, item F11 de la roadmap, utilisera Ctrl+F).
- **Date utilisée : modification seule** (`mtime`, déjà en mémoire, aucun coût de scan).
  - Création : non. Elle est remise à « maintenant » par une copie (un vieux fichier copié paraîtrait neuf), et la stocker ajoute 4 octets à `Node` (48 → 56 octets avec l'alignement, +17 % de mémoire, et casse `node_is_48_bytes`). Elle est pourtant gratuite à lire (`FILE_ID_BOTH_DIR_INFO.creation`) : à ajouter dans F14 (Propriétés) à la demande, pas ici.
  - Dernier accès : non. Windows ne le met à jour que si `fsutil behavior query disablelastaccess` le permet (désactivé ou « géré par le système » selon la version et la taille du volume), et la lecture d'un fichier par un antivirus ou un indexeur le rafraîchit : valeur trompeuse.
  - Le titre de l'histogramme le dit : « Répartition par âge (date de modification) ».
- **Fichiers uniquement.** Les dossiers ne sont ni comptés ni listés (leur date change dès qu'une entrée est ajoutée ou supprimée ; un « âge = max des enfants » demanderait une agrégation de plus, hors périmètre). Les emplacements non reçus (`parent == NONE`) sont ignorés comme dans `top_files`.
- **Tailles dans le mode courant** (`tree.size(id)`, suit Ctrl+T), comme partout ailleurs. Liens physiques : même conséquence que F09 (sur disque, le second nom pèse 0).
- **Portée** : toute l'analyse (id 0). Ni le zoom du treemap, ni la sélection, ni le filtre par nom ne la restreignent.

### Tranches d'âge (bornes exactes)
L'âge se compte en **jours civils locaux** : `jour(t) = fmt::local(t).div_euclid(86_400)`, `aujourd'hui = jour(now)`, `âge = aujourd'hui − jour(mtime)`. Un fichier daté d'aujourd'hui (même plus tard dans la journée) a l'âge 0, ce qui concorde avec « aujourd'hui » de la colonne Modifié (F02). **1 an = 365 jours** (pas d'année civile), 6 mois = 182 jours.

Dix tranches, index fixes, dans l'ordre d'affichage (haut → bas) :

| Index | Libellé | Règle (âge en jours, `t = mtime`) |
|---|---|---|
| 0 | « Date dans le futur » | `jour(t) > aujourd'hui` |
| 1 | « Moins de 30 jours » | `0 ≤ âge < 30` |
| 2 | « 30 jours à 6 mois » | `30 ≤ âge < 182` |
| 3 | « 6 mois à 1 an » | `182 ≤ âge < 365` |
| 4 | « 1 à 2 ans » | `365 ≤ âge < 730` |
| 5 | « 2 à 5 ans » | `730 ≤ âge < 1 826` |
| 6 | « 5 à 10 ans » | `1 826 ≤ âge < 3 652` |
| 7 | « Plus de 10 ans » | `âge ≥ 3 652` et `jour(t) ≥ 1980-01-01 local` |
| 8 | « Avant 1980 (date suspecte) » | `t != 0` et `jour(t) < 1980-01-01 local` (jour 3 652 depuis 1970) |
| 9 | « Date inconnue » | `t == 0` (illisible, ou antérieure à 1970 et bornée à 0 par le scan) |

Priorité des tests : 9, puis 8, puis 0, puis 1 → 7. Le seuil de 1980 est la date minimale FAT/DOS et le repère des horodatages fantaisistes (0, 1970, 1979) ; il est en **heure locale** parce qu'un fichier FAT daté 1980-01-01 00:00 local vaut 1979-12-31 23:00 UTC à Paris et ne doit pas être marqué suspect.

**Pas de `fmt::local` par fichier.** Les seuils sont convertis une fois en secondes UTC, puis chaque `mtime` est comparé à des entiers :

```rust
// src/ui/age.rs
pub const AGE_DAYS: [i64; 6] = [30, 182, 365, 730, 1826, 3652];
const DAY_1980: i64 = 3652; // 1980-01-01, jours depuis 1970-01-01

/// Seuils en secondes Unix UTC. `t >= recent[k]` <=> âge < AGE_DAYS[k] (en jours civils locaux).
pub struct Bounds { pub future: i64, pub recent: [i64; 6], pub y1980: i64 }

/// Minuit local du jour `day` (jours locaux depuis 1970), en secondes UTC.
fn midnight_utc(day: i64, local: &impl Fn(i64) -> i64) -> i64 {
    let lm = day * 86_400;
    let guess = lm - (local(lm) - lm);   // décalage évalué près de l'instant
    lm - (local(guess) - guess)          // corrigé au décalage de l'instant trouvé (heure d'été)
}

/// `now` en secondes Unix UTC ; `local` = `fmt::local` en production, une fermeture dans les tests.
pub fn bounds(now: i64, local: impl Fn(i64) -> i64) -> Bounds
// today = local(now).div_euclid(86_400)
// future = midnight_utc(today + 1)
// recent[k] = midnight_utc(today - (AGE_DAYS[k] - 1))
// y1980 = midnight_utc(DAY_1980)

pub fn bucket(mtime: u32, b: &Bounds) -> usize
// 0 si mtime == 0 -> 9 ; t < y1980 -> 8 ; t >= future -> 0 ; premier k tel que t >= recent[k] -> 1 + k ; sinon 7
```

- `fmt::local` passe de `fn` à `pub(crate) fn` (seul changement dans `src/fmt.rs`). L'appel de production est `age::bounds(now as i64, crate::fmt::local)`.
- Coût : 2 appels `local` par seuil, 8 seuils, une fois par recalcul : négligeable.
- `midnight_utc` est exact pour tout décalage constant autour de minuit, y compris demi-heures (Inde) et changement d'heure à 02:00 / 03:00 (Europe). Seul un fuseau qui change d'heure **à minuit** (minuit inexistant) peut décaler un seuil d'une heure : accepté (voir Risques).

### Histogramme : `AgeView`
Nouveau fichier `src/ui/age.rs` :

```rust
pub const LABELS: [&str; 10] = [...];               // libellés du tableau ci-dessus
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Bucket { pub files: u64, pub size: u64 }

/// Un passage sur l'arbre : fichiers (non dossiers, `parent != NONE`) par tranche, tailles `tree.size(id)`.
/// Les fichiers de taille 0 sont comptés dans `files`.
pub fn age_histogram(tree: &Tree, b: &Bounds) -> [Bucket; 10]

pub struct AgeView { hist: [Bucket; 10], built_version: u64, built: Instant }
impl AgeView {
    pub fn new() -> Self                    // built_version = u64::MAX
    pub fn reset(&mut self)                 // hist à zéro, built_version = u64::MAX
    pub fn show(&mut self, ui: &mut Ui, tree: &Tree, scanning: bool, now: u64)
}
```

- Invariant : `Σ hist[i].size == tree.size(0)` et `Σ hist[i].files == tree.nodes[0].files` une fois le scan terminé (tous les fichiers reçus).
- Recalcul en tête de `show` par la même règle que la vue Top : `top::needs_rebuild(built_version, tree.version, scanning, built.elapsed())` (passer `needs_rebuild` en `pub(super)` dans `top.rs`, ne pas la dupliquer). Recalculer = `bounds(now as i64, fmt::local)` puis `age_histogram`.
- Pas de recalcul à minuit sans nouveau lot ni Ctrl+T : une application laissée ouverte la nuit garde les tranches de la veille jusqu'au prochain changement de `version`. Limite acceptée (commentaire `shortcut:` d'une ligne à côté de l'appel).
- Si `tree.size(0) == 0` : `show` ne dessine **rien** (c'est la liste en dessous qui affiche « Aucune donnée — lancez un scan », pour ne pas répéter le message).

Rendu (`show`), dans le style de `types::show(full = true)` :
1. Ligne de titre (`ui.horizontal`) : « Répartition par âge (date de modification) » en `strong`, puis en `MUTED` 12 px « sur disque » ou « taille logique » selon `tree.on_disk()`, plus « — analyse en cours, chiffres provisoires » si `scanning`. Infobulle du titre : « Âge = jours écoulés depuis la dernière modification, en heure locale. 1 an = 365 jours. Les dossiers ne sont pas comptés. »
2. Une ligne de 24 px par tranche : tranches 1 à 7 **toujours** affichées (même vides : l'échelle reste lisible) ; tranches 0, 8, 9 **seulement si `files > 0`**. Colonnes, de gauche à droite :
   - pastille 10×10 au `painter` : `ACCENT` (1–7), `ORANGE` (0 et 8), `MUTED` (9) ;
   - libellé 13 px `TEXT`, zone de 190 px rognée ;
   - barre : piste `TRACK` (hauteur 6, rayon 3, comme Types), remplissage de la couleur de la pastille, largeur ∝ `size / max(size des tranches affichées)` (aucune barre si le max est 0) ;
   - taille `fmt::size(size)` `mono(12.0)` `TEXT`, alignée à droite, 100 px ;
   - pourcentage `fmt::percent(100 × size / tree.size(0).max(1))` `mono(12.0)` `MUTED`, 64 px ;
   - nombre de fichiers `format!("{} fichiers", fmt::thousands(files))` 12 px `MUTED`, aligné à droite, 130 px (« 1 fichier » au singulier).
   - Survol d'une ligne : infobulle « {files} fichiers, {taille} ». Clic : rien.
3. `ui.add_space(8.0)` puis un filet `BORDER` d'une largeur, avant la liste.

### Liste des plus anciens : `TopView` généralisé
Dans `src/ui/top.rs` :

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum List { Largest, Oldest }
pub fn TopView::new(list: List) -> Self   // remplace new() ; champ `list`
```

| | `List::Largest` (F09, inchangé) | `List::Oldest` (F10) |
|---|---|---|
| Sélection | `top_files(tree, TOP_N)` | `age::oldest_files(tree, TOP_N, b.y1980, b.future)` avec `b = age::bounds(now as i64, fmt::local)` |
| Rang 1 | le plus gros | le plus ancien |
| Tri initial | Taille, desc. | **Modifié, asc.** |
| Titre | « Plus gros fichiers (n) » | « Fichiers les plus anciens (n) » |
| Liste vide (données présentes) | « Aucun fichier de taille non nulle dans ce mode d'affichage » | « Aucun fichier daté de taille non nulle dans ce mode d'affichage » |
| `id_salt` du `ScrollArea`, ids des en-têtes | `"top_rows"`, `("top_col", col)` | `"old_rows"`, `("old_col", col)` (deux instances ne doivent pas partager l'état egui) |

Tout le reste est commun et ne change pas : colonnes, `Layout`, tri par clic (sens initial par colonne inchangé : Modifié démarre en desc. quand on clique dessus depuis une autre colonne), égalités par rang, flèches, `scroll_to`, double-clic = révéler, pied « Total de ces n fichiers : … », message « Aucune donnée — lancez un scan », mention « sur disque » / « taille logique » et « — analyse en cours, liste provisoire ». `rebuild` reçoit `now: u64` en plus (ignoré pour `Largest`) ; `show` le reçoit déjà.

```rust
// src/ui/age.rs
/// Les `n` fichiers les plus anciens de date plausible (`lo <= mtime < hi`, en secondes UTC),
/// de taille > 0 dans le mode courant, triés par mtime croissant puis id croissant.
pub fn oldest_files(tree: &Tree, n: usize, lo: i64, hi: i64) -> Vec<u32>
```
- Même structure que `top_files` : un passage `1..nodes.len()`, garde `!is_dir && parent != NONE && tree.size(id) > 0 && lo <= mtime as i64 && (mtime as i64) < hi` ; **tas-max borné** `BinaryHeap<(u32, u32)>` sur la clé `(mtime, id)` de capacité `n` : `push` tant que `len < n`, sinon remplacer le max si la clé est **strictement** inférieure ; à la fin `into_sorted_vec()` (croissant) et extraction des ids. `n == 0` → vide.
- `mtime == 0` est toujours exclu (`lo = y1980 > 0`). Dates suspectes (avant 1980), inconnues et futures sont donc **hors liste** : elles n'apparaissent que dans l'histogramme (lignes 0, 8, 9).
- Taille 0 exclue (fichiers vides, seconds noms de liens physiques sur disque) : sinon la liste serait remplie de fichiers vides d'installateurs. Conséquence : Ctrl+T peut changer le contenu de la liste.

### Vue, raccourci, onglet, menu
- `View` gagne `Old` ; `View::ALL = [Details, Map, Types, Top, Old]`. `label` = « Fichiers les plus anciens », `tab` = « Anciens », `key` = `Key::F10`, `key_text` = « F10 ». `side_treemap` / `side_types` : vrais pour `Old` (rien à changer dans leur code). Commentaire de `ALL` : « Ordre des onglets et du menu. »
- F10 sans modificateur, tous états de scan, même avec un champ texte focalisé → vue Old (obtenu par `ALL`, aucun code dans `shortcuts`). Maj+F10 / Ctrl+F10 : rien.
- Onglet « Anciens » en 5ᵉ position, infobulle « Fichiers les plus anciens (F10) » (générée par `views::tabs`).
- Menu Affichage : **supprimer** la ligne grisée `item(ui, o, "Fichiers les plus anciens", "", None, true)` ; l'entrée vient de la boucle `view_item`, à la même place (après « 100 plus gros fichiers », avant « Doublons »).
- `App` : champs `age: AgeView` et `old: TopView` (`TopView::new(List::Oldest)`) ; `top` devient `TopView::new(List::Largest)`. `start_scan` appelle aussi `self.age.reset()` et `self.old.reset()` (même raison que F09 : ids et `version` de l'ancien arbre). Branche centrale :
  ```rust
  View::Old => {
      Frame::new().inner_margin(Margin { left: 16, right: 16, top: 0, bottom: 12 }).show(ui, |ui| {
          self.age.show(ui, &self.tree, scanning, now);
          revealed = self.old.show(ui, &self.tree, &mut self.selected, scanning, now);
      });
  }
  ```
  Le traitement de `revealed` existant (sélection, `tv.reveal`, bascule en Détails) sert tel quel.

### Pendant un scan, sans données, cas limites
- `Scanning` : histogramme et liste se remplissent en direct, recalculés au plus toutes les 500 ms chacun, mentions « provisoires ». `Paused` : recalcul dès que la version change. Le scan n'est jamais touché. `Arrêter` : ce qui a été reçu.
- Avant tout scan ou racine de taille 0 : seul « Aucune donnée — lancez un scan » (centré, de la liste).
- Données mais aucun fichier daté de taille > 0 (ex. dossier de fichiers vides, ou tous en 1970) : histogramme affiché, puis en-tête de colonnes et message « Aucun fichier daté de taille non nulle… ».
- Moins de 100 fichiers éligibles : tous, titre avec le nombre réel.
- Nouveau scan, Rafraîchir, Dossier parent, puce, Ctrl+O : `start_scan` → `reset()` des trois vues. La vue reste `Old`.
- Ctrl+T : version → histogramme (tailles) et liste recalculés, tri d'affichage conservé.
- Égalités de date (fréquentes : fichiers d'un même installateur) : ordre par id croissant dans la liste.
- Horloge système en arrière (now < 1980) : les tests de priorité donnent « avant 1980 » ou « futur », pas de panique ; rien à traiter de plus.

Polices et glyphes : uniquement des caractères latins déjà affichés (lettres accentuées, « », —, %, #). **Aucune flèche, aucun emoji, aucun signe ≥ / ≤** dans les libellés ; pastilles et barres au `painter`. Tout autre glyphe doit être vérifié à l'écran.

## Hors périmètre

- Dates de création et de dernier accès, choix de la date par l'utilisateur (F14 affichera création / accès à la demande).
- Âge des dossiers (max ou moyenne des enfants), liste des plus anciens limitée à un dossier, au zoom ou à une tranche (clic sur une barre = filtre), nombre réglable, tranches configurables.
- Liste des fichiers suspects (1970, futur) : seulement comptés.
- Recalcul automatique à minuit, réaction à un changement de fuseau.
- Filtre / recherche par date (F11), sélection multiple (F12), menu contextuel (F13), suppression (F20+), export (F24+).
- Toute modification de `src/scan/`, de `Node` / `Entry` ou de `src/treemap_layout.rs`.

## Fichiers concernés

- `src/ui/mod.rs` : `pub mod age;`, variante `View::Old`.
- `src/ui/views.rs` : `ALL` à 5, branches `Old` ; tests (voir plus bas).
- `src/ui/age.rs` (nouveau) : `AGE_DAYS`, `DAY_1980`, `Bounds`, `midnight_utc`, `bounds`, `bucket`, `LABELS`, `Bucket`, `age_histogram`, `oldest_files`, `AgeView` (`new`, `reset`, `show`) ; tests.
- `src/ui/top.rs` : `pub enum List`, champ `list`, `TopView::new(list)`, sélection / titre / message vide / tri initial / identifiants egui selon `list`, `rebuild(tree, now)`, `needs_rebuild` en `pub(super)` ; tests existants adaptés (`TopView::new(List::Largest)`, `rebuild(&t, 0)`).
- `src/fmt.rs` : `fn local` → `pub(crate) fn local` (rien d'autre).
- `src/ui/menu.rs` : suppression de l'entrée grisée « Fichiers les plus anciens ».
- `src/app.rs` : champs `age`, `old`, `top: TopView::new(List::Largest)` ; `start_scan` → `age.reset()`, `old.reset()` ; branche `View::Old`.
- `README.md`, `ROADMAP.md` (voir Critères de fin).

## Risques

- **Coût de `fmt::local`** : trois appels kernel32 par conversion ; par fichier, 1,3 M conversions coûteraient des centaines de millisecondes par recalcul. D'où `Bounds` : aucune conversion dans les boucles. Ne jamais appeler `fmt::local` ni `fmt::date` dans `age_histogram` / `oldest_files` (`fmt::date` reste appelé pour les ≤ 30 lignes visibles, comme F09).
- **Coût des passages** : deux passages O(N) (histogramme + liste) au plus toutes les 500 ms pendant le scan, et seulement quand la vue Old est affichée. Attendu < 30 ms chacun en release sur 1,3 M de nœuds ; mesurer avec les tests ignorés (Critères de fin). Au-delà, le noter dans « Limites » et passer `STALE_MS` à 1 000 ms ; pas d'incrémental ici.
- **Ids périmés (classe F01)** : `old` contient des ids de l'arbre courant ; `reset()` dans `start_scan` plus la garde `id < nodes.len()` déjà présente dans `TopView::show`. Mettre à jour le commentaire F28 à côté des appels.
- **État egui partagé** entre `top` et `old` : `id_salt` et ids des en-têtes distincts (table ci-dessus), sinon le défilement d'une liste s'applique à l'autre.
- **Changement d'heure à minuit** (fuseaux qui changent à 00:00, ex. Chili, anciennement Brésil) : un seuil peut être décalé d'une heure, donc un fichier modifié dans la première heure d'un jour frontière classé dans la tranche voisine. Accepté (commentaire `shortcut:` dans `midnight_utc`).
- **Règles d'heure d'été historiques** : héritées de F02 (`local` applique la règle courante) ; sans effet sur les tranches sauf à moins d'une heure d'un seuil.
- **Volumes FAT / exFAT** : heures locales converties par Windows avec le décalage courant (voir F02) ; un fichier daté 1980-01-01 00:00 en hiver reste « Plus de 10 ans » grâce au seuil local.
- **F10** : vérifié dans winit 0.30.13 par lecture des sources (pas de `DefWindowProc` pour `WM_SYSKEYDOWN`) ; une mise à jour d'eframe / winit pourrait changer ce comportement : refaire le point 2 de la vérification manuelle à chaque montée de version.
- **Glyphes** : voir la fin de « Comportement attendu ».

## Critères de fin

- `cargo build --release`, `cargo clippy --all-targets -- -D warnings` et `cargo test` passent sans avertissement.
- Tests unitaires sans egui (arbres construits par `Tree::new` + `Tree::ingest`, avec un champ `mtime` renseigné ; **horloge et fuseau injectés** : `now` explicite et fermeture `local`, jamais l'horloge ni le fuseau de la machine). Repères : `NOW = 1_719_835_200` (2024-07-01 12:00 UTC), fuseau fixe `|t| t + 3600` ⇒ aujourd'hui = jour 19 905, `future = 1_719_874_800`, `recent[0] = 1_717_282_800`, `y1980 = 315_529_200`.
  - `src/ui/age.rs` `mod tests` :
    - `bounds_fixed_offset` — avec `|t| t + 3600` et `NOW` : les trois valeurs ci-dessus, et `recent` strictement décroissant.
    - `bounds_half_hour_zone` — `|t| t + 19_800` (Inde) : `future == 19_906 * 86_400 - 19_800`.
    - `midnight_utc_follows_dst` — `local = |t| if t >= 1_711_846_800 { t + 7200 } else { t + 3600 }` (passage à l'heure d'été de Paris, 31 mars 2024 01:00 UTC) : `midnight_utc(19_813)` (31 mars) `== 19_813 * 86_400 - 3600` ; `midnight_utc(19_814)` (1er avril) `== 19_814 * 86_400 - 7200`.
    - `bucket_edges` — avec les bornes de `bounds_fixed_offset` : `0` → 9 ; `1` (1970) → 8 ; `315_529_199` → 8 ; `315_529_200` → 7 ; `1_717_282_800` → 1 ; `1_717_282_799` → 2 ; `1_719_874_799` → 1 ; `1_719_874_800` → 0.
    - `bucket_each_age_range` — pour chaque `k` de 0 à 5, un fichier à midi local du jour `19_905 - AGE_DAYS[k]` (`mtime = jour * 86_400 + 43_200 - 3600`) tombe dans `2 + k`, et un fichier à midi local du jour `19_905 - AGE_DAYS[k] + 1` dans `1 + k`.
    - `histogram_sums_match_root` — une quinzaine de fichiers répartis dans les dix tranches, dans deux dossiers, dont un de taille 0 : `Σ size == tree.size(0)`, `Σ files == tree.nodes[0].files`, les dossiers ne sont pas comptés ; après `set_on_disk(false)` l'égalité tient encore.
    - `histogram_skips_holes` — lot ingéré avec `first` au-delà de la fin : aucun trou compté.
    - `oldest_files_sorted_plausible_only` — fichiers datés 0, 1, 1979, 1985, 2001, futur, un dossier daté 1981, un fichier 1990 de taille 0, avec `lo = 315_529_200` et `hi = 1_719_874_800` : résultat = ids de 1985 puis 2001 seulement.
    - `oldest_files_ties_by_smaller_id` — 4 fichiers de même `mtime` : `oldest_files(t, 2, …) == [1, 2]` ; `n = 0` → vide.
    - `oldest_files_matches_full_sort` — 10 000 fichiers sur 100 dossiers, `mtime` pseudo-aléatoires (LCG déterministe, beaucoup d'égalités, certains hors bornes) : égal au tri complet `(mtime, id)` des éligibles tronqué à 100.
    - `oldest_files_follows_on_disk` — deux noms d'un même `file_id` : sur disque un seul listé, en logique les deux.
    - `age_histogram_bench_1_3m` et `oldest_files_bench_1_3m` — `#[ignore]`, arbre de 1 300 000 fichiers (13 000 dossiers × 100, comme `top_files_bench_1_3m`), durée affichée par `println!`, assertion sur la longueur / la somme seulement. Lancer `cargo test --release bench_1_3m -- --ignored --nocapture` et reporter les durées dans le message de commit.
  - `src/ui/top.rs` : tests existants adaptés ; plus `oldest_list_defaults` — `TopView::new(List::Oldest)` a le tri Modifié asc. ; après `rebuild(&t, NOW)` sur un petit arbre, rangs 1..n dans l'ordre croissant des dates (`now` choisi pour que tous les fichiers soient dans le passé ; le fuseau réel n'influe que sur des bornes à au moins un jour des dates de test).
  - `src/ui/views.rs` :
    - `view_for_key_maps_f6_to_f9` devient `view_for_key_maps_f6_to_f10` (F10 → `Some(View::Old)`).
    - `view_for_key_ignores_other_keys` — remplacer `F10` par `F11` (`F5`, `F11`, `F1`, `A` → `None`).
    - `view_keys_round_trip` — inchangé (couvre la 5ᵉ variante).
    - `side_cards_hide_the_central_one` — ajouter `Old` → treemap et types à droite.
- Vérification manuelle (`cargo run --release`, scans **en lecture seule**, ne jamais cliquer Corbeille / Nettoyer ; aucun fichier créé ni modifié hors d'un dossier neuf sous `%TEMP%`, supprimé à la fin) :
  ```powershell
  $d = New-Item -ItemType Directory "$env:TEMP\optisize-f10-$(Get-Random)"
  function New-Dated($n, $date, $kb) { $f = "$d\$n"; [IO.File]::WriteAllBytes($f, (New-Object byte[] ($kb * 1024))); (Get-Item $f).LastWriteTime = $date }
  New-Dated 'jour.bin' (Get-Date) 8
  New-Dated 'an.bin' (Get-Date).AddDays(-400) 16
  New-Dated 'vieux.bin' (Get-Date '2001-05-01') 32
  New-Dated 'fat1980.bin' (Get-Date '1980-01-01 00:00') 64
  New-Dated 'epoque.bin' ([DateTime]::new(1970,1,1,0,0,5,'Utc').ToLocalTime()) 4
  New-Dated 'futur.bin' (Get-Date '2035-01-01') 4
  New-Dated 'vide.bin' (Get-Date '1999-01-01') 0
  ```
  1. Lancer sans argument : F10 → « Aucune donnée — lancez un scan », pas d'histogramme.
  2. Focus dans le champ « Dossier » : F10 change de vue, sans modifier le texte et **sans activer de menu** (la touche suivante, par exemple une flèche, agit normalement). Idem avec F6–F9 pour comparer.
  3. Scanner `$d` : histogramme — « Moins de 30 jours » 1 fichier (8 Ko), « 1 à 2 ans » 1, « Plus de 10 ans » 3 (`vieux`, `fat1980`, `vide`), « Avant 1980 (date suspecte) » 1 (`epoque`), « Date dans le futur » 1 ; les tranches 30 jours à 6 mois, 6 mois à 1 an, 2 à 5 ans, 5 à 10 ans affichées vides ; pas de ligne « Date inconnue ». Somme des tailles = total de l'arbre.
  4. Liste : `fat1980.bin`, `vieux.bin`, `an.bin`, `jour.bin` dans cet ordre, rangs 1 à 4 ; ni `epoque`, ni `futur`, ni `vide`. Titre « Fichiers les plus anciens (4) ».
  5. `cargo run --release -- C:\Users` : pendant le scan, F10 → histogramme et liste se remplissent, mentions provisoires, interface fluide, scan non interrompu. Les fichiers « 1 janv. 1970 » vus dans la vue F09 sont comptés en « Avant 1980 », absents de la liste.
  6. Tri par chaque colonne (deux clics), F9 puis F10 : chaque liste garde son propre tri et son propre défilement.
  7. Clic, flèches, Entrée (Explorateur), double-clic (révéler en Détails) : comme F09.
  8. Ctrl+T : tailles de l'histogramme et en-tête « Taille » / « Sur disque » changent, nombres de fichiers non.
  9. Menu Affichage : « Fichiers les plus anciens » actif avec « F10 », surligné dans la vue ; plus d'entrée grisée. Onglet « Anciens » en 5ᵉ position.
  10. Vue Old affichée, puis Ctrl+R, une puce de lecteur, Ctrl+O : chaque scan vide puis remplit, sans panique.
  11. Aucun carré vide (glyphe manquant) dans l'histogramme, l'en-tête, les lignes, le pied.
  12. Supprimer le seul dossier `$d` créé ci-dessus (`Remove-Item -Recurse $d`).
- Lignes à mettre à jour : statut F10 → `done` dans `ROADMAP.md` ; section « État » du `README.md`, « Fonctionnel » : ajouter « vue Fichiers les plus anciens (F10 : répartition par âge de la date de modification en heure locale, dates avant 1980 et futures à part, 100 plus anciens triables, suit Ctrl+T) » ; « Limites » : tranches non recalculées à minuit sans nouveau lot, tout point de Risques constaté (durées au-delà de 30 ms, F10 intercepté), ou « vue F10 : pas encore vérifiée à l'écran » si la vérification manuelle n'a pas pu être faite.
