---
id: F03
title: Embarquer la police JetBrains Mono pour les tailles
model: sonnet        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Câblage d'interface borné à un appel egui documenté ici (fichier, URL, empreinte, code), sans donnée utilisateur en jeu ; une erreur se voit tout de suite à l'écran et dans les tests.
depends_on: []
---

## Contexte

Les tailles, pourcentages et compteurs sont déjà dessinés dans la famille egui `FontFamily::Monospace`, via `theme::mono(size)` (`src/ui/theme.rs`). Mais aucune police n'est enregistrée par l'application : `Monospace` est donc la police par défaut d'egui (`Hack-Regular`, embarquée par la fonctionnalité `default_fonts` d'eframe). La maquette et le README (« TODO : … JetBrains Mono embarquée ») prévoient JetBrains Mono, comme TreeSize aligne ses colonnes de chiffres.

Appels actuels de `mono(...)` (tous en bénéficieront sans être modifiés) :

| Fichier | Texte |
|---|---|
| `src/ui/tree_view.rs` (`let m = mono(12.0)`) | colonnes Taille, %, Fichiers de l'arbre |
| `src/ui/drives.rs` | « 120,4 Go / 476,9 Go » des puces de lecteur (`mono(11.0)`) |
| `src/ui/status.rs` (`mono_label`) | « N fichiers · N dossiers », « 3,2 s · N fichiers/s », « N dossiers ignorés » |
| `src/ui/types.rs` | tailles de la liste Types (`mono(12.0)`) |
| `src/ui/side.rs` | « N récupérables » (carte Nettoyage) et chemin de la sélection |
| `src/ui/toolbar.rs` | champ « Dossier » (`mono(13.0)`) |
| `src/ui/theme.rs` | `TextStyle::Monospace` (aucun widget ne l'utilise aujourd'hui) |

API egui 0.37 vérifiée dans `epaint-0.37.0` / `egui-0.37.0` : `Context::add_font(FontInsert)` ajoute une police **en gardant** les polices par défaut ; `FontInsert::new(name, FontData::from_static(&'static [u8]), vec![InsertFontFamily { family, priority: FontPriority::Highest }])` la place en tête de la famille. epaint 0.37 met en forme le texte avec `harfrust` (ligatures possibles), d'où le choix de la variante **NL** (sans ligatures) ci-dessous.

## Choix de la police

- **Version** : JetBrains Mono **v2.304** (dernière publiée, 14 janvier 2023), dépôt officiel https://github.com/JetBrains/JetBrainsMono.
- **Un seul fichier** : `fonts/ttf/JetBrainsMonoNL-Regular.ttf` (variante « No Ligatures », graisse Regular), **208 576 octets**, empreinte git (blob SHA-1) `70d2ec9e28d6e06e923629b78b8a12c12e841589`.
  - NL et non `JetBrainsMono-Regular.ttf` (273 900 o) : pas de ligature possible (`->`, `!=`, `>=` dans un chemin ou un nom affiché dans le champ Dossier) et 65 Ko de moins.
  - Pas de gras, d'italique, ni de police variable : l'application n'utilise qu'une graisse.
- **Licence** : SIL Open Font License 1.1, fichier `OFL.txt` du même tag, 4 399 octets, empreinte git `8bee4148c1d54dbf5dae6d6c117fc80414266abb`, première ligne « Copyright 2020 The JetBrains Mono Project Authors (https://github.com/JetBrains/JetBrainsMono) ». L'OFL autorise l'embarquement dans un logiciel, y compris commercial, à condition de garder la notice de copyright et la licence avec la distribution ; elle interdit de vendre la police seule (sans objet ici).
- **Téléchargement** (fichiers bruts du tag, plus simple que d'extraire l'archive `JetBrainsMono-2.304.zip` de 5,6 Mo) :
  ```powershell
  New-Item -ItemType Directory -Force assets\fonts | Out-Null
  Invoke-WebRequest https://raw.githubusercontent.com/JetBrains/JetBrainsMono/v2.304/fonts/ttf/JetBrainsMonoNL-Regular.ttf -OutFile assets\fonts\JetBrainsMonoNL-Regular.ttf
  Invoke-WebRequest https://raw.githubusercontent.com/JetBrains/JetBrainsMono/v2.304/OFL.txt -OutFile assets\fonts\JetBrainsMono-OFL.txt
  git hash-object assets\fonts\JetBrainsMonoNL-Regular.ttf   # doit afficher 70d2ec9e28d6e06e923629b78b8a12c12e841589
  git hash-object assets\fonts\JetBrainsMono-OFL.txt         # doit afficher 8bee4148c1d54dbf5dae6d6c117fc80414266abb
  ```
  Si une empreinte diffère, **s'arrêter** et le signaler (fichier altéré ou mauvaise URL) ; ne pas committer le fichier.

## Comportement attendu

- Au démarrage, JetBrains Mono NL devient la **première** police de `FontFamily::Monospace`. Tous les textes du tableau ci-dessus s'affichent en JetBrains Mono, sans modifier leurs appels.
- Les polices par défaut restent derrière dans la chaîne de repli (`Hack`, puis icônes / emoji, puis repli système d'egui) : un caractère absent de JetBrains Mono (idéogrammes, arabe, emoji dans un chemin affiché dans le champ Dossier ou le panneau Sélection) s'affiche quand même, dans la police suivante. Aucun carré « tofu » de plus qu'aujourd'hui.
- La famille `Proportional` (noms de fichiers, en-têtes, boutons, menus, étiquettes du treemap, infobulles, dates) **ne change pas**.
- « Arbre en mémoire : 42,0 Mo » dans la barre d'état (`src/ui/status.rs`, seul affichage de taille encore en proportionnel hors treemap) passe en mono : remplacer son `ui.label(RichText::new(...).color(MUTED))` par `.font(mono(12.0))` comme `mono_label`, sans autre changement de mise en page.
- Les étiquettes du treemap (« nom (12,3 Go) ») et son infobulle **restent** en proportionnel : le nom domine le texte, le passer en mono élargirait les étiquettes et en masquerait davantage.
- La police est chargée depuis le binaire (`include_bytes!`) : aucun fichier lu au lancement, l'exécutable reste autonome et portable.
- Pas de nouvelle dépendance dans `Cargo.toml` ; `default_fonts` reste activé (fournit Hack et Ubuntu-Light, nécessaires au repli et au texte proportionnel).

## Code

Dans `src/ui/theme.rs` :

```rust
use eframe::egui::{FontData, FontInsert, FontPriority, InsertFontFamily};

/// Nom de la police dans les définitions egui (clé de `font_data`).
pub const MONO_FONT: &str = "JetBrainsMonoNL-Regular";
/// JetBrains Mono NL v2.304, SIL OFL 1.1 (assets/fonts/JetBrainsMono-OFL.txt).
const MONO_TTF: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNL-Regular.ttf");

/// Place JetBrains Mono en tête de la famille Monospace, polices par défaut gardées en repli.
fn install_fonts(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        MONO_FONT,
        FontData::from_static(MONO_TTF),
        vec![InsertFontFamily { family: FontFamily::Monospace, priority: FontPriority::Highest }],
    ));
}
```

et appeler `install_fonts(ctx)` en première ligne de `pub fn apply(ctx)` (déjà appelée une fois dans `App::new`, `src/app.rs`). Si l'un de ces types n'est pas réexporté par `eframe::egui`, l'importer depuis `eframe::egui::epaint::text` (même type) ; ne pas ajouter `epaint` à `Cargo.toml`.

Choix « famille Monospace » plutôt qu'une famille dédiée `FontFamily::Name("jetbrains")` : tous les textes concernés passent déjà par `mono()`, donc zéro appel à modifier, et le champ texte du Dossier reste cohérent. Une famille dédiée n'apporterait rien tant qu'aucun texte ne doit rester en Hack.

Pas de `FontTweak` par défaut. Seulement si la vérification manuelle montre les chiffres visiblement décalés en hauteur par rapport aux noms sur la même ligne de l'arbre, ajouter `.tweak(FontTweak { y_offset_factor: ..., ..Default::default() })` sur `FontData` avec la plus petite valeur qui corrige, et le noter dans le commit.

## Hors périmètre

- Changer la police proportionnelle (Ubuntu-Light / Segoe UI), les tailles de texte, la mise en page ou les largeurs de colonnes.
- Graisses gras / italique, police variable, ligatures.
- Désactiver `default_fonts` ou retirer Hack pour gagner de la place (Hack sert de repli).
- Afficher la licence dans l'interface (fenêtre À propos : F18) ou l'installer avec le binaire (installeur : F37, qui devra copier `assets/fonts/JetBrainsMono-OFL.txt`).
- Option utilisateur pour choisir la police (F17).

## Fichiers concernés

- **Nouveaux** : `assets/fonts/JetBrainsMonoNL-Regular.ttf` (binaire, 208 576 o) et `assets/fonts/JetBrainsMono-OFL.txt` (licence, inchangée). Pas besoin de `.gitattributes` : git détecte le `.ttf` comme binaire.
- `src/ui/theme.rs` : `MONO_FONT`, `MONO_TTF`, `install_fonts`, appel dans `apply`, tests.
- `src/ui/status.rs` : « Arbre en mémoire » en `mono(12.0)`.
- `README.md` : section « État », retirer « JetBrains Mono embarquée » du TODO, ajouter « tailles et compteurs en JetBrains Mono embarquée » dans « Fonctionnel », et une ligne « Police : JetBrains Mono (SIL OFL 1.1, `assets/fonts/JetBrainsMono-OFL.txt`) ».
- Aucun autre fichier de code ; `Cargo.toml` inchangé.

## Risques

- **Taille du binaire** : + 208 576 octets (police non compressée dans `.rdata`), soit environ + 0,2 Mo sur les 6,8 Mo actuels de `target/release/optisize.exe`. Relever la taille avant / après et la donner dans le rapport ; un écart de plus de 300 Ko signale un mauvais fichier.
- **Chemin de `include_bytes!`** : relatif au fichier source (`src/ui/theme.rs` → `../../assets/fonts/...`). Un mauvais chemin casse la compilation (erreur visible, pas silencieuse).
- **Fichier altéré** : vérifier les deux empreintes git avant de committer (voir plus haut). Un fichier qui n'est pas une police ferait paniquer egui au premier rendu ; le test `mono_font_bytes_are_truetype` le détecte avant.
- **Alignement vertical** : métriques (ascendante / descendante) différentes de Hack ; les colonnes sont alignées par `Align2::RIGHT_CENTER`, donc un décalage d'un pixel est possible mais pas de chevauchement. Voir la règle `FontTweak` ci-dessus.
- **Largeur** : JetBrains Mono a une chasse de 600/1000 em, Hack ≈ 0,602 em : les colonnes de l'arbre ne changent pas de largeur, « 1 023,9 Go » tient comme avant.
- **Glyphes** : toutes les chaînes produites par `fmt::size`, `fmt::thousands`, `fmt::percent` sont en ASCII (chiffres, virgule, espace ordinaire, `%`, `o Ko Mo Go To`), couvertes par JetBrains Mono. Les chemins non latins passent par le repli (voir Comportement attendu).
- **Ordre de mise en place** : `add_font` s'applique au début du prochain passage, ou tout de suite si aucun texte n'a encore été mis en page ; l'appel dans `App::new` (avant le premier passage) suffit. Ne pas l'appeler à chaque frame (`add_font` ne fait rien si le nom existe déjà, mais c'est inutile).

## Critères de fin

- `cargo build --release`, `cargo clippy --all-targets` et `cargo test` passent sans avertissement.
- Tests dans `src/ui/theme.rs`, `#[cfg(test)] mod tests` :
  - `mono_font_bytes_are_truetype` — `MONO_TTF.len() == 208_576` et `MONO_TTF[..4] == [0x00, 0x01, 0x00, 0x00]` (en-tête sfnt TrueType).
  - `jetbrains_mono_heads_monospace` — `let ctx = egui::Context::default(); apply(&ctx); let _ = ctx.run_ui(Default::default(), |_| {});` puis, dans `ctx.fonts(|f| ...)`, `f.definitions().families[&FontFamily::Monospace][0] == MONO_FONT`.
  - `default_fonts_kept_as_fallback` — même mise en place : la famille `Monospace` contient encore `"Hack"` (après `MONO_FONT`), et la première police de `FontFamily::Proportional` n'est **pas** `MONO_FONT`.
  - `mono_font_digits_same_width` — même mise en place, puis `ctx.fonts_mut(|f| f.layout_no_wrap(s.to_owned(), mono(12.0), Color32::WHITE).size().x)` (API vérifiée dans epaint 0.37) : la largeur de `"1 111,1 Go"` est égale (à 0,01 près) à celle de `"8 888,8 Go"` (chiffres à chasse fixe).
- Vérification manuelle (`cargo run --release`), scans en **lecture seule** uniquement (aucun clic sur Nettoyer, Corbeille ou Examiner et nettoyer) :
  1. Avant de modifier le code, faire une capture d'écran de référence (arbre + puces + barre d'état) sur un scan de `C:\Windows\Fonts` ou `$env:USERPROFILE\Documents`.
  2. Après : même scan. Les colonnes Taille, %, Fichiers, les puces « X Go / Y Go », la liste Types, la barre d'état (compteurs, débit, « Arbre en mémoire ») et le champ Dossier sont en JetBrains Mono (comparer avec la capture de référence : la forme des chiffres change, hauteur d'x plus grande) ; les noms de fichiers, menus et boutons sont inchangés.
  3. Les chiffres d'une colonne restent alignés à droite, sans décalage vertical gênant avec le nom sur la même ligne.
  4. Taper dans le champ Dossier un chemin contenant `->` et `!=` (sans valider avec Entrée) : les caractères restent séparés (pas de ligature), puis effacer le texte.
  5. Relever la taille de `target/release/optisize.exe` avant / après (attendu : environ + 0,2 Mo).
- Lignes à mettre à jour : statut F03 → `done` dans `ROADMAP.md` ; section « État » du `README.md` comme indiqué dans « Fichiers concernés ».
